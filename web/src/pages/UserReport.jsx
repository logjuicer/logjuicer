/* global setTimeout, clearTimeout, localStorage */
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
import { withRouter } from 'react-router-dom'
import {
  Button, Col, Grid, HelpBlock,
  Form, FormControl, FormGroup, Row
} from 'patternfly-react'

import { submitRequest } from '../api'


class UserReportPage extends React.Component {
  static propTypes = {
    history: PropTypes.object.isRequired
  }

  state = {}

  save = () => {
    const data = this.getFormData()
    if (!data.uuid) {
      return
    }
    localStorage.setItem('lastReport', JSON.stringify(data))
    submitRequest(data)
      .then(() => {
        this.props.history.push('/status')
      })
      .catch(error => {
        throw (error)
      })
  }

  cancel = () => {
    this.name.value = ''
    this.reporter.value = ''
    this.build.value = ''
    this.url.value = ''
    this.path.value = ''
  }

  getFormData = () => {
    const data = {
      name: this.name.value,
      reporter: this.reporter.value,
      uuid: this.build.value,
      path: this.path.value,
    }
    if (this.url.value) {
      data.url = this.url.value
    }
    return data
  }

  componentDidMount () {
    try {
      const data = JSON.parse(localStorage.getItem('lastReport'))
      // test data
      this.name.value = data.name
      this.reporter.value = data.reporter
      this.build.value = data.uuid
      this.path.value = data.path
      if (data.url) {
        this.url.value = data.url
      }
    } catch (e) {
      console.log('Couldn\'t read lastReport', e)
    }
  }

  render () {
    const Zuuls = [
      ['Local', 'http://localhost/zuul/api/tenant/local'],
      ['RDO',
       'https://softwarefactory-project.io/zuul/api/tenant/rdoproject.org'],
      ['Software Factory',
       'https://softwarefactory-project.io/zuul/api/tenant/local'],
      ['OpenStack',
       'https://zuul.openstack.org/api'],
    ]
    return (
      <Grid>
        <h2>Report a new build to be analyzed</h2>
        <p>Use the form bellow to report a Zuul build and trigger an automated
        analyzes</p>
        <hr />
        <Form horizontal>
          <FormGroup controlId='name'>
            <Col sm={3}>
              Anomaly name
            </Col>
            <Col sm={9}>
              <FormControl type='text' inputRef={i => this.name = i} />
              <HelpBlock>
{'Enter a description like "periodic job failed" or "kernel panic"'}
              </HelpBlock>
            </Col>
          </FormGroup>
          <FormGroup controlId='reporter'>
            <Col sm={3}>
              Reporter
            </Col>
            <Col sm={9}>
              <FormControl type='text' inputRef={i => this.reporter = i}/>
              <HelpBlock>
                {'Enter your name like "IRC nick" or "Email address"'}
              </HelpBlock>
            </Col>
          </FormGroup>
          <FormGroup controlId='build'>
            <Col sm={3}>
              Target build
            </Col>
            <Col sm={9}>
              <FormControl type='text' inputRef={i => this.build = i} />
              <HelpBlock>
                Enter the build uuid
              </HelpBlock>
            </Col>
          </FormGroup>
          <FormGroup controlId='path'>
            <Col sm={3}>
              Include path
            </Col>
            <Col sm={9}>
              <FormControl type='text' inputRef={i => this.path = i} />
              <HelpBlock>
                {'Enter artifacts path to include like "logs/"'}
              </HelpBlock>
            </Col>
          </FormGroup>
          <FormGroup controlId='build'>
            <Col sm={3}>
              Build store
            </Col>
            <Col sm={9}>
              <FormControl
                componentClass='select'
                type='select' inputRef={i => this.url = i}>
                {Zuuls.map((item, idx) => (
                  <option key={idx} value={item[1]}>{item[0]}</option>
                ))}
              </FormControl>
              <HelpBlock>
                Those are known Zuul API endpoints to query build information.
              </HelpBlock>
            </Col>
          </FormGroup>
          <Row style={{paddingTop: '10px',paddingBottom: '10px'}}>
            <Col smOffset={3} sm={9}>
              <span>
                <Button bsStyle='primary' onClick={this.save}>
                  Save
                </Button>
              </span>
              <span>
                <Button onClick={this.cancel}>
                  Cancel
                </Button>
              </span>
            </Col>
          </Row>
        </Form>
      </Grid>
    )
  }
}

export default withRouter(UserReportPage)
