// Copyright 2018 Red Hat, Inc
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may
// not use this file except in compliance with the License. You may obtain
// a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.

import React from 'react'
import PropTypes from 'prop-types'
import {
  Button,
  Form,
  FormGroup,
  Col,
  Row,
  HelpBlock,
  FormControl,
  Icon,
  Modal
} from 'patternfly-react'

import { updateAnomaly } from '../../api'
import { fetchAnomalyAction } from '../../reducers'

class CopyAnomalyModal extends React.Component {
  static propTypes = {
    anomaly: PropTypes.object.isRequired,
    logfile: PropTypes.object
  }

  state = {
    show: false
  }

  submit = () => {
    const anomaly = this.props.anomaly.props.anomaly
    const data = {
      'name': this.newName.value,
      'reporter': this.reporter.value
    }
    if (this.props.logfile) {
      data.logfiles = [this.props.logfile.id]
    }
    updateAnomaly(anomaly.uuid, {copy: data})
      .then(response => {
        if (response.data.error) {
          console.log('Oops', response.data.error)
        } else {
          this.setState({show: false})
          this.props.anomaly.history.push('/view/' + response.data.anomalyId)
          this.props.anomaly.dispatch(
            fetchAnomalyAction(response.data.anomalyId))
        }
      })
      .catch(error => {
        throw (error)
      })
  }

  render () {
    const { anomaly, logfile } = this.props
    return (
      <Modal show={this.state.show}
             onShow={() => {
               this.newName.value = anomaly.props.anomaly.name + ' (copy)'
               this.reporter.value = anomaly.props.anomaly.reporter}}
             onHide={() => {this.setState({show: false})}}>
        <Modal.Header>
          <button
            className='close'
            onClick={() => {this.setState({show: false})}}
            aria-hidden='true'
            aria-label='Close'
            >
            <Icon type='pf' name='close' />
          </button>
          <Modal.Title>
            Duplicate the report
          </Modal.Title>
        </Modal.Header>
        <Modal.Body>
          <Form horizontal>
            <FormGroup controlId='name'>
              <Col sm={3}>
                Anomaly name
              </Col>
              <Col sm={9}>
                <FormControl type='text' inputRef={i => this.newName = i} />
                  <HelpBlock>
                    Enter a new name
                  </HelpBlock>
              </Col>
            </FormGroup>
            <FormGroup controlId='reporter'>
              <Col sm={3}>
                Reporter
              </Col>
              <Col sm={9}>
                <FormControl type='text' inputRef={i => this.reporter = i} />
                  <HelpBlock>
                    Enter your name
                  </HelpBlock>
              </Col>
            </FormGroup>
            {logfile && (
            <FormGroup controlId='logfile'>
              <Col sm={3}>
                Logfile
              </Col>
              <Col sm={9}>
                <FormControl type='text' value={logfile.path} disabled />
                  <HelpBlock>
                    Logfile selected
                  </HelpBlock>
              </Col>
            </FormGroup>
            )}
            <Row style={{paddingTop: '10px',paddingBottom: '10px'}}>
              <Col smOffset={3} sm={9}>
                <span>
                  <Button bsStyle='primary' onClick={this.submit}>
                    Copy
                  </Button>
                </span>
                <span>
                  <Button onClick={() => {this.setState({show: false})}}>
                    Cancel
                  </Button>
                </span>
              </Col>
            </Row>
          </Form>
        </Modal.Body>
      </Modal>
    )
  }
}

export default CopyAnomalyModal
